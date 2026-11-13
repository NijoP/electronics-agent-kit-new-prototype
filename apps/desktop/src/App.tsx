import { useState } from "react";
import { ModelSettings } from "./ModelSettings";
import { ProjectHome } from "./ProjectHome";
import type { Snapshot } from "./types";
import { Workspace } from "./Workspace";

export function App() {
  const [opened, setOpened] = useState<Snapshot | null>(null);
  const [settings, setSettings] = useState(false);
  return (
    <>
      {opened ? (
        <Workspace key={opened.project.id} initial={opened} onClosed={() => setOpened(null)} />
      ) : (
        <ProjectHome onOpened={setOpened} />
      )}
      <button type="button" className="settings-fab" onClick={() => setSettings(true)}>
        Model settings
      </button>
      {settings && <ModelSettings onClose={() => setSettings(false)} />}
    </>
  );
}
