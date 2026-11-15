// Which terminal space belongs to the project open in the app. Sessions inherit EAK_PROJECT_DIR /
// EAK_PROJECT_ID from their space, so an agent started in another project's space edits THAT
// project and its changes never appear in the open editors.
import type { Space } from "./api";

export interface OpenProject { dir: string; name: string; id: string }

/** Folder paths compare equal regardless of separators, a trailing separator and (on Windows
 * drive paths) letter case. */
export function sameDir(a: string, b: string): boolean {
  const norm = (p: string) => {
    let s = p.trim().replace(/\\/g, "/").replace(/\/+$/, "");
    if (/^[a-zA-Z]:/.test(s)) s = s.toLowerCase();
    return s;
  };
  return norm(a) === norm(b);
}

/** The space bound to `project`: by project id first, else a space rooted at its folder. */
export function spaceForProject(spaces: Space[], project: OpenProject): Space | null {
  return spaces.find((s) => s.project_id === project.id) ?? spaces.find((s) => sameDir(s.root, project.dir)) ?? null;
}

/** True when `space` is tied to a project other than the open one (a plain space such as Home,
 * with no project, never is). */
export function boundToOtherProject(space: Space | null, project: OpenProject | null): boolean {
  if (!space || !project) return false;
  if (space.project_id) return space.project_id !== project.id;
  return false;
}
