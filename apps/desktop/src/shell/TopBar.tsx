// Top bar of the app shell (product vision §1): the EAK wordmark on its own with its full name,
// Electronics Agent Kit, then the active project. On the right, the "+" of a registered workspace
// that can create a new space (the Terminal Workspace), shown only while one provides it.
import { formatShortcut } from "../design/shortcuts";

interface Props {
  /** Active project's name and folder, or null on Home with nothing open. */
  project: { name: string; dir: string | null } | null;
  /** A registered workspace's "new space" action, or null when none provides one. */
  create: { label: string; shortcut: string; run: () => void } | null;
}

export function TopBar({ project, create }: Props) {
  return (
    <header className="appbar topbar">
      <span className="brand">EAK</span>
      <span className="brand-name">Electronics Agent Kit</span>
      {project && (
        <span className="crumb">
          <span className="crumb-sep" aria-hidden="true">
            /
          </span>
          <span className="appbar-project" title={project.dir ?? undefined}>
            {project.name}
          </span>
        </span>
      )}
      <span className="spacer" />
      {create && (
        <button type="button" className="app-new" aria-label={create.label} title={`${create.label} (${formatShortcut(create.shortcut)})`} onClick={create.run}>
          +
        </button>
      )}
    </header>
  );
}
