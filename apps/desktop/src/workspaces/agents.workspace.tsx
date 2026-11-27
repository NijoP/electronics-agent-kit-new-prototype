// Registers the Agents workspace (M2.1, ADR-0058) as a top-level app workspace (../shell/registry.ts).
// The navigation rail shows it in the vision's place, after Library (App.tsx, shell/ActivityBar.tsx);
// Model settings stay in the Settings menu and in the workspace's context bar.
import { AgentsWorkspace } from "../agents/AgentsWorkspace";
import { AGENTS_AREA } from "../shell/ActivityBar";
import { Icons } from "../shell/icons";
import { registerWorkspace } from "../shell/registry";

registerWorkspace({
  id: AGENTS_AREA,
  label: "Agents workspace",
  navLabel: "Agents",
  icon: Icons.sparkle,
  order: 5,
  render: (ctx) => (
    <AgentsWorkspace
      active={ctx.active}
      projectId={ctx.projectId}
      projectName={ctx.projectName}
      projectDir={ctx.projectDir}
      openModelSettings={ctx.openModelSettings}
      showWorkspace={ctx.showWorkspace}
    />
  ),
});
