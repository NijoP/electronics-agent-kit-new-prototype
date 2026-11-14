// Registers the Terminal Workspace as a top-level app workspace (see ../shell/registry.ts).
// The "+" in the app bar bumps a nonce that the workspace turns into "open or create the current
// project's space".
import { useSyncExternalStore } from "react";
import { registerWorkspace } from "../shell/registry";
import { TerminalWorkspace } from "../terminal/TerminalWorkspace";

let nonce = 0;
const listeners = new Set<() => void>();
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};
const bump = () => {
  nonce += 1;
  listeners.forEach((l) => l());
};

function Terminal({ active, project }: { active: boolean; project: { dir: string; name: string; id: string } | null }) {
  const newSpaceNonce = useSyncExternalStore(subscribe, () => nonce);
  return <TerminalWorkspace visible={active} newSpaceNonce={newSpaceNonce} project={project} />;
}

registerWorkspace({
  id: "terminal",
  label: "Terminal Workspace",
  order: 10,
  shortcut: "Mod+Shift+T",
  newLabel: "New terminal space",
  onNew: bump,
  render: (ctx) => (
    <Terminal
      active={ctx.active}
      project={ctx.projectDir && ctx.projectId ? { dir: ctx.projectDir, name: ctx.projectName ?? "project", id: ctx.projectId } : null}
    />
  ),
});
