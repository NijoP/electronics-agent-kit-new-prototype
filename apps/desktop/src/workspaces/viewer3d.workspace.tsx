// Registers the 3D board viewer as an EDA view next to Schematic and PCB (see ../shell/registry.ts).
// The shell opens it as a document tab (slice C6) from its activity-bar button, Mod+3 (first
// registered EDA view) or the palette, and gives it the 3D action set (menus, toolbar).
import { registerWorkspace } from "../shell/registry";
import { Icons } from "../shell/icons";
import { Viewer3D } from "../viewer3d/Viewer3D";

registerWorkspace({
  id: "viewer3d",
  label: "3D",
  placement: "eda",
  icon: Icons.viewer3d,
  order: 10,
  render: (ctx) => (
    <Viewer3D
      projectId={ctx.projectId}
      revision={ctx.revision}
      active={ctx.active}
      selected={ctx.selected}
      onSelect={ctx.select}
      onProbe={ctx.probeTo}
      execute={ctx.execute}
      busy={ctx.busy}
      layerColourOf={ctx.layerColour}
      layerHidden={ctx.layerHidden}
      onToggleLayer={ctx.toggleLayer}
    />
  ),
});
