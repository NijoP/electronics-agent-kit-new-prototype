import { useEffect, useState } from "react";
import { applyTheme, loadTheme, type Theme } from "./design/theme";
import { ModelSettings } from "./ModelSettings";
import { ProjectHome } from "./ProjectHome";
import type { Snapshot } from "./types";
import { Workspace } from "./Workspace";

export function App() {
  const [opened, setOpened] = useState<Snapshot | null>(null);
  const [settings, setSettings] = useState(false);
  const [theme, setTheme] = useState<Theme>(loadTheme);
  useEffect(() => applyTheme(theme), [theme]);
  const toggleTheme = () => setTheme((t) => (t === "dark" ? "light" : "dark"));
  return (
    <>
      {opened ? (
        <Workspace
          key={opened.project.id}
          initial={opened}
          onClosed={() => setOpened(null)}
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
      {settings && <ModelSettings onClose={() => setSettings(false)} />}
    </>
  );
}
