// Registers the 3D board viewer as an EDA view next to Schematic and PCB (see ../shell/registry.ts).
// The shell gives it an activity-bar button, Mod+3 (first registered EDA view) and a palette entry.
import { registerWorkspace } from "../shell/registry";
import { Icons } from "../shell/icons";
import { Viewer3D } from "../viewer3d/Viewer3D";

registerWorkspace({
  id: "viewer3d",
  label: "3D",
  placement: "eda",
  icon: Icons.viewer3d,
  order: 10,
  render: (ctx) => <Viewer3D projectId={ctx.projectId} revision={ctx.revision} active={ctx.active} />,
});
