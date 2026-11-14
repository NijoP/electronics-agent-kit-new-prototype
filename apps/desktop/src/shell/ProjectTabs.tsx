// Strip of the projects open in this window. Each tab is a separate engine project: switching makes
// it the active one (every command, file operation and editor then acts on it alone) and closing a
// tab closes only that project. Document tabs, panel layout and viewports are kept per project.
import type { OpenProject } from "../types";

interface Props {
  projects: OpenProject[];
  onSwitch: (id: string) => void;
  onClose: (id: string) => void;
  onOpen: () => void;
  disabled?: string;
}

export function ProjectTabs({ projects, onSwitch, onClose, onOpen, disabled }: Props) {
  return (
    <div className="tabs project-tabs" role="tablist" aria-label="Open projects">
      {projects.map((p) => (
        <span key={p.id} className={p.active ? "doc-tab on" : "doc-tab"}>
          <button
            type="button"
            role="tab"
            aria-selected={p.active}
            className="tab-label"
            title={p.dir ?? p.name}
            disabled={!!disabled && !p.active}
            onClick={() => !p.active && onSwitch(p.id)}
          >
            {p.name}
          </button>
          <button type="button" className="tab-close" aria-label={`Close project ${p.name}`} title={disabled ?? `Close project ${p.name}`} disabled={!!disabled} onClick={() => onClose(p.id)}>
            ×
          </button>
        </span>
      ))}
      <button type="button" className="icon" aria-label="Open another project" title={disabled ?? "Open another project alongside these"} disabled={!!disabled} onClick={onOpen}>
        +
      </button>
    </div>
  );
}
