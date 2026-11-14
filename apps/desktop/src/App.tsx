import { useEffect, useState } from "react";
import { applyTheme, loadTheme, type Theme } from "./design/theme";
import { ModelSettings } from "./ModelSettings";
import { ProjectHome } from "./ProjectHome";
import { term } from "./terminal/api";
import { TerminalWorkspace } from "./terminal/TerminalWorkspace";
import type { Snapshot } from "./types";
import { Workspace } from "./Workspace";

type Env = "eda" | "terminal";

export function App() {
  const [opened, setOpened] = useState<Snapshot | null>(null);
  const [settings, setSettings] = useState(false);
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const [env, setEnv] = useState<Env>("eda");
  // The terminal workspace mounts on first use and then stays mounted, so its sessions and
  // screens survive switching back to EDA.
  const [terminalUsed, setTerminalUsed] = useState(false);
  const [newSpace, setNewSpace] = useState(0);
  useEffect(() => {
    applyTheme(theme);
  }, [theme]);
  // The active project is where new terminal spaces start; existing sessions never move.
  const dir = opened?.dir ?? null;
  const name = opened?.project.name ?? null;
  const pid = opened?.project.id ?? null;
  useEffect(() => {
    void term.setContext(dir, name, pid).catch(() => {});
  }, [dir, name, pid]);
  const showTerminal = () => {
    setTerminalUsed(true);
    setEnv("terminal");
  };
  const toggleTheme = () => setTheme((t) => (t === "dark" ? "light" : "dark"));
  return (
    <>
      <nav className="env-bar" aria-label="Workspaces">
        <strong>EAK</strong>
        <button type="button" className={env === "eda" ? "env on" : "env"} aria-pressed={env === "eda"} onClick={() => setEnv("eda")}>
          Project / EDA
        </button>
        <button type="button" className={env === "terminal" ? "env on" : "env"} aria-pressed={env === "terminal"} onClick={showTerminal}>
          Terminal Workspace
        </button>
        <button
          type="button"
          className="env"
          aria-label="New terminal space"
          title={opened ? `New terminal space for ${opened.project.name}` : "Open the terminal workspace"}
          onClick={() => {
            showTerminal();
            setNewSpace((n) => n + 1);
          }}
        >
          +
        </button>
      </nav>
      <div className="env-host" hidden={env !== "eda"}>
        {opened ? (
          <Workspace
            key={opened.project.id}
            initial={opened}
            onClosed={() => setOpened(null)}
            onOpened={setOpened}
            onSettings={() => setSettings(true)}
            theme={theme}
            onTheme={toggleTheme}
          />
        ) : (
          <>
            <ProjectHome onOpened={setOpened} />
            <div className="home-fab">
              <button type="button" onClick={toggleTheme} title="Toggle light/dark theme">
                {theme === "dark" ? "Light theme" : "Dark theme"}
              </button>
              <button type="button" className="settings-fab" onClick={() => setSettings(true)}>
                Model settings
              </button>
            </div>
          </>
        )}
      </div>
      {terminalUsed && (
        <div className="env-host" hidden={env !== "terminal"}>
          <TerminalWorkspace
            visible={env === "terminal"}
            newSpaceNonce={newSpace}
            project={opened?.dir ? { dir: opened.dir, name: opened.project.name, id: opened.project.id } : null}
          />
        </div>
      )}
      {settings && <ModelSettings onClose={() => setSettings(false)} />}
    </>
  );
}
