import { useState } from "react";
import { ProjectHome } from "./ProjectHome";
import type { Snapshot } from "./types";
import { Workspace } from "./Workspace";

export function App() {
  const [opened, setOpened] = useState<Snapshot | null>(null);
  return opened ? (
    <Workspace key={opened.project.id} initial={opened} onClosed={() => setOpened(null)} />
  ) : (
    <ProjectHome onOpened={setOpened} />
  );
}
