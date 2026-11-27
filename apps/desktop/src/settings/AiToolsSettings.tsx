// Settings › AI tools (remap W2-E, Traycer review §11.3 item 8): one row per coding-agent CLI
// found on this computer, with the facts its own commands report (found / version / signed in /
// connected to this project / reason) and real actions: Connect and Disconnect register or remove
// EAK's MCP server through the tool's own CLI, Sign in runs the tool's own login in a tab of the
// existing Terminal workspace, and a session starts there or headless. A state EAK cannot read is
// shown as "unknown", never as "yes"; a tool that is not installed is "Not found".
import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
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
} from "./aiTools";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));
/** How often the session list refreshes while the page is open. */
const SESSION_POLL_MS = 3000;

export function AiToolsSettings() {
  const [rows, setRows] = useState<ToolStatus[] | null>(null);
  const [probing, setProbing] = useState(false);
  const [projectOpen, setProjectOpen] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [prompt, setPrompt] = useState("");
  const [surface, setSurface] = useState<Surface>("terminal");
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [logFor, setLogFor] = useState<string | null>(null);
  const [log, setLog] = useState<string>("");

  const probe = useCallback(async () => {
    setProbing(true);
    setError(null);
    try {
      setProjectOpen(await api.currentProject().then(() => true, () => false));
      setRows(await api.agentTools());
    } catch (e) {
      setError(`Could not check the AI tools: ${message(e)}`);
    } finally {
      setProbing(false);
    }
  }, []);

  const loadSessions = useCallback(async () => {
    try {
      setSessions(await api.agentSessions());
    } catch (e) {
      setError(`Could not list the sessions: ${message(e)}`);
    }
  }, []);

  useEffect(() => {
    void probe();
    void loadSessions();
    const t = window.setInterval(() => void loadSessions(), SESSION_POLL_MS);
    return () => window.clearInterval(t);
  }, [probe, loadSessions]);

  const showLog = useCallback(async (id: string) => {
    setLogFor(id);
    try {
      setLog(await api.agentSessionLog(id, 200));
    } catch (e) {
      setLog(`Could not read the log: ${message(e)}`);
    }
  }, []);

  const shownState = sessions.find((s) => s.id === logFor)?.state;
  // Live tail: while the shown session runs, its log refreshes with the session list.
  useEffect(() => {
    if (!logFor || shownState !== "running") return undefined;
    const t = window.setInterval(() => void showLog(logFor), SESSION_POLL_MS);
    return () => window.clearInterval(t);
  }, [logFor, shownState, showLog]);

  /** Run one action; a returned row replaces the tool's row, a session refreshes the list. */
  const act = async (key: string, run: () => Promise<string | ToolStatus | AgentSession>) => {
    setBusy(key);
    setError(null);
    setNotice(null);
    try {
      const out = await run();
      if (typeof out === "string") setNotice(out);
      else if ("can_connect" in out) {
        setRows((rs) => rs?.map((r) => (r.id === out.id ? out : r)) ?? null);
        setNotice(`${out.label}: connected to this project: ${connectedText(out)}`);
      } else {
        setNotice(`${out.label} session started (${out.surface === "terminal" ? "a new tab in the Terminal workspace" : "headless, log below"})`);
        await loadSessions();
        if (out.surface === "headless") void showLog(out.id);
      }
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(null);
    }
  };

  const shownSession = sessions.find((s) => s.id === logFor) ?? null;

  return (
    <div className="ai-tools">
      <h3>AI tools</h3>
      <p className="muted">
        Coding agents installed on this computer. EAK uses each tool's own sign-in and never stores its keys. Connect registers EAK's MCP server (eak-mcp) with the tool for the open project, so the agent works
        through EAK's engine and checks.
      </p>
      <div className="row">
        <button type="button" onClick={() => void probe()} disabled={probing} title={probing ? "Checking…" : undefined}>
          {probing ? "Checking…" : "Check again"}
        </button>
        {!projectOpen && rows && <span className="muted">No project is open: Connect and sessions need one.</span>}
      </div>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      {!rows && !error && <p className="muted">Checking the AI tools on this computer…</p>}
      {rows && (
        <table className="settings-table" aria-label="AI tools">
          <thead>
            <tr>
              <th>Tool</th>
              <th>Installed</th>
              <th>Signed in</th>
              <th>Connected to this project</th>
              <th>Details</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((t) => {
              const connect = connectBlocker(t, projectOpen);
              const login = loginBlocker(t);
              const start = launchBlocker(t, surface, prompt, projectOpen);
              const disconnect = t.connected === true ? null : "Not connected to this project";
              return (
                <tr key={t.id} data-tool={t.id}>
                  <th scope="row">{t.label}</th>
                  <td>{foundText(t)}</td>
                  <td title={t.auth_detail ?? undefined}>{signedInText(t)}</td>
                  <td title={t.connection_scope ?? undefined}>{connectedText(t)}</td>
                  <td className="muted">{reasonText(t) ?? t.auth_detail ?? ""}</td>
                  <td className="actions">
                    <button
                      type="button"
                      disabled={!!connect || busy !== null}
                      title={connect ?? t.connection_scope ?? undefined}
                      onClick={() => void act(`connect:${t.id}`, () => api.agentToolConnect(t.id))}
                    >
                      Connect
                    </button>
                    <button type="button" disabled={!!disconnect || busy !== null} title={disconnect ?? undefined} onClick={() => void act(`disconnect:${t.id}`, () => api.agentToolDisconnect(t.id))}>
                      Disconnect
                    </button>
                    <button
                      type="button"
                      disabled={!!login || busy !== null}
                      title={login ?? undefined}
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
                      disabled={!!start || busy !== null}
                      title={start ?? undefined}
                      onClick={() => void act(`launch:${t.id}`, () => api.agentToolLaunch(t.id, surface, prompt.trim() || null))}
                    >
                      Launch session
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      <fieldset className="ai-tools-launch">
        <legend>New session</legend>
        <label className="inline">
          <input type="radio" name="ai-surface" checked={surface === "terminal"} onChange={() => setSurface("terminal")} /> In a terminal tab
        </label>
        <label className="inline">
          <input type="radio" name="ai-surface" checked={surface === "headless"} onChange={() => setSurface("headless")} /> Headless (log shown here)
        </label>
        <p className="muted">Sessions started here run EAK's harness in propose mode: design changes come back as proposals for you to review and apply.</p>
        <label>
          First message (optional in a terminal tab)
          <input type="text" aria-label="First message" value={prompt} onChange={(e) => setPrompt(e.target.value)} placeholder="e.g. Review the power rails" />
        </label>
      </fieldset>
      <h4>Sessions</h4>
      {sessions.length === 0 ? (
        <p className="muted">No AI session started from EAK yet (sessions of earlier runs are kept).</p>
      ) : (
        <table className="settings-table" aria-label="AI sessions">
          <thead>
            <tr>
              <th>Tool</th>
              <th>Task</th>
              <th>Where</th>
              <th>Status</th>
              <th>Last activity</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {sessions.map((s) => {
              const last = s.events[s.events.length - 1];
              const stop = s.state === "running" ? null : "The session is not running";
              return (
                <tr key={s.id} data-session={s.id}>
                  <th scope="row">{s.label}</th>
                  <td>
                    <code>{s.id}</code>
                  </td>
                  <td>{s.surface === "terminal" ? "Terminal tab" : "Headless"}</td>
                  <td title={s.detail}>
                    {SESSION_STATE_LABEL[s.state]}
                    {s.exit_code !== null ? ` (exit ${s.exit_code})` : ""}
                  </td>
                  <td className="muted">{last ? eventText(last) : s.detail}</td>
                  <td className="actions">
                    <button type="button" onClick={() => void showLog(s.id)}>
                      Show log
                    </button>
                    <button
                      type="button"
                      disabled={!!stop || busy !== null}
                      title={stop ?? undefined}
                      onClick={() => void act(`stop:${s.id}`, () => api.agentToolStop(s.id).then((v) => `${v.label} session: ${SESSION_STATE_LABEL[v.state]}`))}
                    >
                      Stop
                    </button>
                    <button
                      type="button"
                      disabled={busy !== null}
                      title={s.state === "running" ? "Stop it and start it again with the same first message" : "Start it again with the same first message"}
                      onClick={() => void act(`restart:${s.id}`, () => api.agentSessionRestart(s.id))}
                    >
                      Restart
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      {shownSession && (
        <section aria-label="Session log">
          <h4>
            Log · {shownSession.label} · {SESSION_STATE_LABEL[shownSession.state]}
          </h4>
          {shownSession.log_file && <p className="muted">Full log: {shownSession.log_file}</p>}
          <button type="button" onClick={() => void showLog(shownSession.id)}>
            Refresh log
          </button>
          {shownSession.state === "running" && <span className="muted"> Updates every {SESSION_POLL_MS / 1000} s while the session runs.</span>}
          <pre className="ai-tools-log" style={{ maxHeight: 240, overflow: "auto", whiteSpace: "pre-wrap" }}>{log || "(no output yet)"}</pre>
        </section>
      )}
    </div>
  );
}
