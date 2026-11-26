# Workspace modules

Drop a file named `<name>.workspace.tsx` in this folder and the shell imports it at start-up. It
registers a workspace through `../shell/registry.ts`; nobody edits the shell.

Two placements:

* **`placement: "app"` (default)** — a top-level area of the navigation rail, after the EDA
  areas (the Terminal Workspace; `navLabel` gives the rail a shorter name and `icon` its button).
  It fills the main area beside the rail. Give it `onNew` and the top bar shows the `+` button (and
  `Ctrl/Cmd+Shift+T`) that creates a new space in it.
* **`placement: "eda"`** — an independent view inside the EDA workspace next to Schematic and PCB
  (the 3D board viewer). It gets a navigation-rail button (`icon`) and fills the editor area.

```tsx
import { registerWorkspace } from "../shell/registry";
import { FileExplorer } from "../shell/FileExplorer";

registerWorkspace({
  id: "terminal", // unique; "editor", "schematic", "pcb", "explorer" are reserved
  label: "Terminal Workspace",
  newLabel: "New terminal space",
  onNew: () => createSpace(),
  render: (ctx) => <TerminalWorkspace active={ctx.active} projects={ctx.openProjects} />,
});
```

Rules the shell relies on:

* A registered workspace is mounted the first time it is opened and then **stays mounted
  (hidden)** while another is shown, so switching never kills a terminal process. `ctx.active` says
  whether it is visible. While another workspace is shown the EDA shortcuts and editors are off, so
  keystrokes typed into a terminal never reach them.
* `ctx.projectId` / `ctx.projectDir` are the active project; `ctx.openProjects` lists every open
  project. A terminal space should **copy** the folder at creation and keep it: opening another
  project must not redirect existing sessions.
* Only register an entry that opens something real; never a placeholder button.
* The explorer is reusable: `<FileExplorer projectId={...} />` takes an open project's id or
  `dir:<absolute path>` for any folder, shows git status and the branch, can mark a working
  directory (`cwd`) and offers "Use as working directory" when `onCwd` is given. All file
  operations go through the sandboxed `project_file*` IPC commands (`src-tauri/src/workspace.rs`).
